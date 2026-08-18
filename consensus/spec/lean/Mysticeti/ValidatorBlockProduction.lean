/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorBlockSync

namespace Mysticeti

/-! Block production from validator-local exact-next transitions.

The first part gives one executable finite schedule. The second part proves two
results for every trace that follows the stated local rules and fair actions:

* Each correct, available validator stores and sends own blocks at unbounded
  rounds.
* Every finite consecutive correct-block window becomes complete. This result is
  for the stalled-commit proof.

The proofs derive parent readiness from earlier local production, retained-owner
block synchronization, and acceptance. They do not take a quorum layer, parent
availability, or a common round as an input.

The block-sync storage rule keeps a needed source block until synchronization
finishes. The current model states this as monotone retention. A GC refinement
must replace this strong rule with protected retention for each active sync goal.
-/

/-! ### Pointwise production and acceptance -/

/-- Pointwise production by all correct, available validators gives a quorum
layer. The quorum stake is derived from the fixed fault bounds. -/
theorem every_correct_available_validator_produced_gives_quorum_layer
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat)
    (produced :
      EveryCorrectAvailableValidatorProduced faults world round) :
    ProducedCorrectQuorumLayer config faults world round := by
  have liveSubset : VoterSet.SubsetAt config.authorityCount
      faults.correctAvailable
      (VoterSet.inter faults.correctAvailable (world.producedAuthors round)) := by
    intro validator validatorInRange validatorLive
    have validatorProduced := produced validator validatorInRange validatorLive
    simp [VoterSet.inter, ValidatorWorldState.producedAuthors, validatorLive,
      validatorProduced]
  have liveStake := faults.correct_available_stake_is_quorum
  have layerStake := weight_mono config.stake liveSubset
  exact Nat.le_trans liveStake layerStake

/-- Pointwise acceptance by every correct, available validator gives one common
accepted quorum layer. -/
theorem every_correct_available_validator_accepted_gives_common_layer
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat)
    (accepted :
      EveryCorrectAvailableValidatorAccepted faults world round) :
    CommonAcceptedCorrectQuorumLayer config faults world round := by
  intro observer observerInRange observerLive
  have liveSubset : VoterSet.SubsetAt config.authorityCount
      faults.correctAvailable
      (VoterSet.inter faults.correctAvailable
        (world.acceptedAuthors observer round)) := by
    intro author authorInRange authorLive
    have authorAccepted := accepted observer author observerInRange observerLive
      authorInRange authorLive
    simp [VoterSet.inter, ValidatorWorldState.acceptedAuthors, authorLive,
      authorAccepted]
  have liveStake := faults.correct_available_stake_is_quorum
  have layerStake := weight_mono config.stake liveSubset
  exact Nat.le_trans liveStake layerStake

/-! ### Concrete blocks for the executable schedule -/

/-- The executable schedule identifies a block by its author and round. -/
abbrev ScheduledBlockId := Nat × Nat

/-- The canonical block reference for one author and round. -/
def scheduledBlockRef (author round : Nat) :
    ValidatorBlockRef ScheduledBlockId where
  id := (author, round)
  author := author
  round := round

/-- Correct, available validator indices in canonical order. -/
def scheduledCorrectValidators
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config) : List Nat :=
  (List.range config.authorityCount).filter
    (fun validator => faults.correctAvailable validator = true)

/-- A scheduled non-genesis block names one parent from each correct, available
validator. Genesis blocks have no parents. -/
def scheduledBlock
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (author round : Nat) : ValidatorBlock ScheduledBlockId where
  reference := scheduledBlockRef author round
  parents := if 0 < round then
    (scheduledCorrectValidators config faults).map
      (fun parentAuthor => scheduledBlockRef parentAuthor (round - 1))
  else
    []

/-- A correct, available validator occurs in the canonical validator list. -/
theorem mem_scheduled_correct_validators
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    {validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorLive : faults.correctAvailable validator = true) :
    validator ∈ scheduledCorrectValidators config faults := by
  simp [scheduledCorrectValidators, validatorInRange, validatorLive]

/-- The canonical validator list has no repeated validator. -/
theorem scheduled_correct_validators_nodup
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config) :
    (scheduledCorrectValidators config faults).Nodup := by
  exact (List.nodup_range : (List.range config.authorityCount).Nodup).filter _

/-- A scheduled block names at most one parent branch from each author. -/
theorem scheduled_block_parent_authors_nodup
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (author round : Nat) :
    (scheduledBlock config faults author round).ParentAuthorsNodup := by
  by_cases positiveRound : 0 < round
  · simpa [ValidatorBlock.ParentAuthorsNodup, scheduledBlock, positiveRound,
      scheduledBlockRef, List.map_map, Function.comp_def] using
      scheduled_correct_validators_nodup config faults
  · simp [ValidatorBlock.ParentAuthorsNodup, scheduledBlock, positiveRound]

/-- All scheduled non-genesis parents are in the immediate preceding round. -/
theorem scheduled_block_parents_are_immediate
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (author round : Nat) :
    (scheduledBlock config faults author round).ParentsAreImmediate := by
  intro parent parentInList
  by_cases positiveRound : 0 < round
  · simp only [scheduledBlock, positiveRound, if_pos, List.mem_map] at parentInList
    rcases parentInList with ⟨parentAuthor, _, parentIsCanonical⟩
    subst parent
    change (round - 1) + 1 = round
    omega
  · simp [scheduledBlock, positiveRound] at parentInList

/-- Each correct, available validator occurs in the scheduled parent-author set. -/
theorem correct_available_subset_scheduled_parent_authors
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (author round : Nat)
    (positiveRound : 0 < round) :
    VoterSet.SubsetAt config.authorityCount faults.correctAvailable
      (scheduledBlock config faults author round).parentAuthors := by
  intro parentAuthor parentInRange parentLive
  have parentInList := mem_scheduled_correct_validators config faults
    parentInRange parentLive
  simp [ValidatorBlock.parentAuthors, scheduledBlock, positiveRound,
    scheduledBlockRef, parentInList]

/-- The immediate parents of each scheduled non-genesis block have quorum stake.
This is derived from the fixed fault bounds. -/
theorem scheduled_block_has_quorum_immediate_parents
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (author round : Nat)
    (positiveRound : 0 < round) :
    (scheduledBlock config faults author round).HasQuorumImmediateParents config := by
  refine ⟨scheduled_block_parent_authors_nodup config faults author round,
    scheduled_block_parents_are_immediate config faults author round, ?_⟩
  have liveStake := faults.correct_available_stake_is_quorum
  have parentStake := weight_mono config.stake
    (correct_available_subset_scheduled_parent_authors config faults author round
      positiveRound)
  exact Nat.le_trans liveStake parentStake

/-! ### Executable exact-next schedule -/

/-- A block is present in the scheduled local state when its author is correct and
available and its round is not later than the schedule height. -/
def scheduledOwnBlock
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (height validator round : Nat) : Option (ValidatorBlockRef ScheduledBlockId) :=
  if validator < config.authorityCount ∧
      faults.correctAvailable validator = true ∧ round ≤ height then
    some (scheduledBlockRef validator round)
  else
    none

/-- One observer's canonical accepted representative. -/
def scheduledAcceptedBlock
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (height observer round author : Nat) :
    Option (ValidatorBlockRef ScheduledBlockId) :=
  if observer < config.authorityCount ∧
      faults.correctAvailable observer = true ∧
      author < config.authorityCount ∧
      faults.correctAvailable author = true ∧ round ≤ height then
    some (scheduledBlockRef author round)
  else
    none

/-- The local state after the finite schedule has completed all rounds through
`height`. The state contains only pointwise author and observer results. -/
def scheduledLocalState
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    (height validator : Nat) : ValidatorLocalState ScheduledBlockId CommitId :=
  let live := validator < config.authorityCount ∧
    faults.correctAvailable validator = true
  { clock := height
    commitHead := { index := 0, id := initialCommitId, round := 0 }
    installedCommitAt := fun index =>
      if index = 0 then some initialCommitId else none
    commitInstallSourceAt := fun _ => none
    lastCommitTime := 0
    thresholdClockRound := height + 1
    highestSignedRound := if live then height else 0
    ownBlockAt := scheduledOwnBlock config faults height validator
    sentOwnBlockAt := fun round => decide (live ∧ round ≤ height)
    accepted := fun reference => decide
      (live ∧ reference.author < config.authorityCount ∧
        faults.correctAvailable reference.author = true ∧
        reference.round ≤ height ∧
        reference.id = (reference.author, reference.round))
    retained := fun reference => decide
      (live ∧ reference.author < config.authorityCount ∧
        faults.correctAvailable reference.author = true ∧
        reference.round ≤ height ∧
        reference.id = (reference.author, reference.round))
    requested := fun _ => false
    acceptedRepresentative :=
      scheduledAcceptedBlock config faults height validator
    recoveryParentChoice :=
      scheduledAcceptedBlock config faults height validator
    gcRound := 0
    recovery := if live then some
      { baselineCommit := { index := 0, id := initialCommitId, round := 0 }
        targetRound := height + 1
        parentsReadyAt := some height
        deadline := some height
        alignmentWitness := none }
      else none
    committer := { pendingRounds := [] }
    observedPeerCommit := fun _ => none }

/-- The complete finite schedule state. The catalog is ghost history. It does not
make a block available unless the related local-state field also contains it. -/
def scheduledWorld
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    (height : Nat) : ValidatorWorldState ScheduledBlockId CommitId Unit where
  epochActive := true
  validatorState := scheduledLocalState config faults initialCommitId height
  blockCatalog := fun blockId =>
    if blockId.1 < config.authorityCount ∧ blockId.2 ≤ height then
      some (scheduledBlock config faults blockId.1 blockId.2)
    else
      none
  packets := fun _ => none

/-- Every correct, available validator has produced each scheduled round. -/
theorem scheduled_world_has_every_correct_producer
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    {height round : Nat}
    (roundInSchedule : round ≤ height) :
    EveryCorrectAvailableValidatorProduced faults
      (scheduledWorld config faults initialCommitId height) round := by
  intro validator validatorInRange validatorLive
  simp [scheduledWorld, scheduledLocalState, scheduledOwnBlock,
    validatorInRange, validatorLive, roundInSchedule]

/-- Every correct, available observer has accepted every correct, available
author's block in each scheduled round. -/
theorem scheduled_world_has_every_correct_acceptance
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    {height round : Nat}
    (roundInSchedule : round ≤ height) :
    EveryCorrectAvailableValidatorAccepted faults
      (scheduledWorld config faults initialCommitId height) round := by
  intro observer author observerInRange observerLive authorInRange authorLive
  simp [scheduledWorld, scheduledLocalState, scheduledAcceptedBlock,
    observerInRange, observerLive, authorInRange, authorLive, roundInSchedule]

/-- One validator's local parent guard. This predicate describes one local DAG
view. It does not state that another validator has the same parents. -/
def LocalCorrectParentQuorum
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (validator parentRound : Nat) : Prop :=
  config.thresholds.quorum ≤
    weight config.authorityCount config.stake
      (VoterSet.inter faults.correctAvailable
        (world.acceptedAuthors validator parentRound))

/-- The scheduled local view has a quorum parent guard at each completed round. -/
theorem scheduled_world_has_local_parent_quorum
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    {height parentRound validator : Nat}
    (parentRoundInSchedule : parentRound ≤ height)
    (validatorInRange : validator < config.authorityCount)
    (validatorLive : faults.correctAvailable validator = true) :
    LocalCorrectParentQuorum config faults
      (scheduledWorld config faults initialCommitId height) validator
      parentRound := by
  have accepted := scheduled_world_has_every_correct_acceptance config faults
    initialCommitId parentRoundInSchedule
  have common :=
    every_correct_available_validator_accepted_gives_common_layer config faults
      (scheduledWorld config faults initialCommitId height) parentRound accepted
  exact common validator validatorInRange validatorLive

/-- One exact-next proposal effect at one correct, available validator. All fields
refer only to that validator's target, local parent view, and durable result. -/
structure LocalExactNextProposalEffect
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (before after : ValidatorWorldState BlockId CommitId PacketId)
    (validator targetRound : Nat) : Prop where
  validatorInRange : validator < config.authorityCount
  validatorCorrectAvailable : faults.correctAvailable validator = true
  targetIsExactNext : before.nextRecoveryRound validator = targetRound
  localParentQuorum :
    LocalCorrectParentQuorum config faults before validator (targetRound - 1)
  durableOwnBlock :
    ((after.validatorState validator).ownBlockAt targetRound).isSome = true
  sentOwnBlock :
    (after.validatorState validator).sentOwnBlockAt targetRound = true
  signerRoundAdvanced :
    (after.validatorState validator).highestSignedRound = targetRound
  earlierOwnBlocksPersist :
    ∀ round, round < targetRound →
      (after.validatorState validator).ownBlockAt round =
        (before.validatorState validator).ownBlockAt round

/-- One local acceptance effect for one observer and one author. -/
structure LocalBlockAcceptanceEffect
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (after : ValidatorWorldState BlockId CommitId PacketId)
    (observer author round : Nat) : Prop where
  observerInRange : observer < config.authorityCount
  observerCorrectAvailable : faults.correctAvailable observer = true
  authorInRange : author < config.authorityCount
  authorCorrectAvailable : faults.correctAvailable author = true
  authorBlockDurable :
    ((after.validatorState author).ownBlockAt round).isSome = true
  observerAccepted :
    ((after.validatorState observer).acceptedRepresentative round author).isSome =
      true

/-- The next scheduled world is obtained through a valid local exact-next effect
for each correct, available validator. -/
theorem scheduled_round_has_local_exact_next_effect
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    (height validator : Nat)
    (validatorInRange : validator < config.authorityCount)
    (validatorLive : faults.correctAvailable validator = true) :
    LocalExactNextProposalEffect config faults
      (scheduledWorld config faults initialCommitId height)
      (scheduledWorld config faults initialCommitId (height + 1))
      validator (height + 1) := by
  constructor
  · exact validatorInRange
  · exact validatorLive
  · simp [ValidatorWorldState.nextRecoveryRound, scheduledWorld,
      scheduledLocalState, validatorInRange, validatorLive]
  · have parents := scheduled_world_has_local_parent_quorum config faults
      initialCommitId (height := height) (parentRound := height)
      (validator := validator) (Nat.le_refl _) validatorInRange validatorLive
    simpa using parents
  · simp [scheduledWorld, scheduledLocalState, scheduledOwnBlock,
      validatorInRange, validatorLive]
  · simp [scheduledWorld, scheduledLocalState, validatorInRange, validatorLive]
  · simp [scheduledWorld, scheduledLocalState, validatorInRange, validatorLive]
  · intro round roundBeforeTarget
    have roundBeforeOrAtHeight : round ≤ height := by omega
    have roundBeforeOrAtNext : round ≤ height + 1 := by omega
    simp [scheduledWorld, scheduledLocalState, scheduledOwnBlock,
      validatorInRange, validatorLive, roundBeforeOrAtHeight,
      roundBeforeOrAtNext]

/-- The next scheduled world contains the local acceptance effect for each pair
of correct, available validators. -/
theorem scheduled_round_has_local_acceptance_effect
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    (height observer author : Nat)
    (observerInRange : observer < config.authorityCount)
    (observerLive : faults.correctAvailable observer = true)
    (authorInRange : author < config.authorityCount)
    (authorLive : faults.correctAvailable author = true) :
    LocalBlockAcceptanceEffect config faults
      (scheduledWorld config faults initialCommitId (height + 1))
      observer author (height + 1) := by
  constructor
  · exact observerInRange
  · exact observerLive
  · exact authorInRange
  · exact authorLive
  · exact scheduled_world_has_every_correct_producer config faults
      initialCommitId (Nat.le_refl _) author authorInRange authorLive |>.1
  · exact scheduled_world_has_every_correct_acceptance config faults
      initialCommitId (Nat.le_refl _) observer author observerInRange observerLive
        authorInRange authorLive

/-- Every scheduled non-genesis round uses a valid concrete parent list for each
correct, available author. -/
theorem scheduled_round_blocks_have_valid_local_parent_guard
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (round author : Nat)
    (positiveRound : 0 < round) :
    (scheduledBlock config faults author round).HasQuorumImmediateParents config :=
  scheduled_block_has_quorum_immediate_parents config faults author round
    positiveRound

/-! ### Finite windows and unbounded scheduled production -/

/-- At every finite schedule height, all rounds through that height contain all
correct, available producers, all correct-to-correct acceptances, and both derived
quorum properties. -/
theorem scheduled_world_has_complete_round
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (initialCommitId : CommitId)
    {height round : Nat}
    (roundInSchedule : round ≤ height) :
    EveryCorrectAvailableValidatorProduced faults
          (scheduledWorld config faults initialCommitId height) round ∧
      EveryCorrectAvailableValidatorAccepted faults
          (scheduledWorld config faults initialCommitId height) round ∧
      ProducedCorrectQuorumLayer config faults
          (scheduledWorld config faults initialCommitId height) round ∧
      CommonAcceptedCorrectQuorumLayer config faults
          (scheduledWorld config faults initialCommitId height) round := by
  have produced := scheduled_world_has_every_correct_producer config faults
    initialCommitId roundInSchedule
  have accepted := scheduled_world_has_every_correct_acceptance config faults
    initialCommitId roundInSchedule
  exact ⟨produced, accepted,
    every_correct_available_validator_produced_gives_quorum_layer config faults
      (scheduledWorld config faults initialCommitId height) round produced,
    every_correct_available_validator_accepted_gives_common_layer config faults
      (scheduledWorld config faults initialCommitId height) round accepted⟩

/-- The executable exact-next schedule gives every correct, available validator
own blocks at unbounded rounds. -/
theorem scheduled_trace_has_block_production_liveness
    {CommitId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage ScheduledBlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (initialCommitId : CommitId) :
    BlockProductionLiveness config faults network
      (fun height => scheduledWorld config faults initialCommitId height) := by
  intro validator start minimumRound validatorInRange validatorLive _afterGst
    _activeEpoch
  let finish := max (start + 1) minimumRound
  refine ⟨finish, finish, ?_, ?_, ?_⟩
  · exact Nat.le_trans (Nat.le_add_right _ 1) (Nat.le_max_left _ _)
  · exact Nat.le_max_right _ _
  · constructor
    · change (if validator < config.authorityCount ∧
          faults.correctAvailable validator = true then start else 0) < finish
      rw [if_pos ⟨validatorInRange, validatorLive⟩]
      have nextAtMost : start + 1 ≤ finish := by
        exact Nat.le_max_left _ _
      exact Nat.lt_of_lt_of_le (Nat.lt_succ_self start) nextAtMost
    · exact scheduled_world_has_every_correct_producer config faults
        initialCommitId (Nat.le_refl _) validator validatorInRange validatorLive

/-! ### All fair traces -/

/-- The block-sync projection of one validator world. The request cursor is local
scheduler state that is not part of the consensus state. -/
def blockSyncTraceOfWorld
    {BlockId CommitId PacketId : Type}
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (requestCursor : Time → Nat → Nat) :
    BlockSyncTrace (ValidatorBlockRef BlockId) :=
  fun time validator =>
    { retained := ((trace time).validatorState validator).retained
      needed := ((trace time).validatorState validator).requested
      accepted := ((trace time).validatorState validator).accepted
      requestCursor := requestCursor time validator }

/-- One proposal task can create the exact-next block or finish sending a block
that the same task already made durable. -/
def ExactNextProposalEnabledAt
    {CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId))
    (validator targetRound time : Nat) : Prop :=
  (trace time).epochActive = true ∧
    validator < config.authorityCount ∧
    faults.correctAvailable validator = true ∧
    ((((trace time).validatorState validator).ownBlockAt targetRound = none ∧
        (trace time).nextRecoveryRound validator = targetRound ∧
        LocalCorrectParentQuorum config faults (trace time) validator
          (targetRound - 1)) ∨
      (((trace time).validatorState validator).ownBlockAt targetRound =
          some (scheduledBlockRef validator targetRound) ∧
        ((trace time).validatorState validator).sentOwnBlockAt targetRound = false))

/-- Local state and transition rules used by the fair-trace theorem.

Every field is about one validator, one author, or one observer. No field states
that a quorum layer, common round, parent synchronization, or later block exists. -/
structure ExactNextBlockProductionRules
    {CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)) : Prop where
  /-- Correct validators use the canonical exact-next block reference. -/
  ownBlockIsCanonical : ∀ time validator round reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).ownBlockAt round = some reference →
    reference = scheduledBlockRef validator round
  /-- If the durable successor is absent, the current durable own block is the
  local signer floor. This is an adjacent no-skip rule. It does not retain block
  history from genesis. -/
  missingSuccessorKeepsSignerAtCurrent : ∀ time validator round,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).ownBlockAt round =
      some (scheduledBlockRef validator round) →
    ((trace time).validatorState validator).ownBlockAt (round + 1) = none →
    ((trace time).validatorState validator).highestSignedRound = round
  /-- Durable own blocks persist while the epoch is active. -/
  ownBlockPersists : ∀ validator round reference earlier later,
    earlier ≤ later →
    (∀ time, earlier ≤ time → time ≤ later →
      (trace time).epochActive = true) →
    ((trace earlier).validatorState validator).ownBlockAt round = some reference →
    ((trace later).validatorState validator).ownBlockAt round = some reference
  /-- A sent own block has durable local data. -/
  sentOwnBlockRequiresOwnBlock : ∀ time validator round,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).sentOwnBlockAt round = true →
    ((trace time).validatorState validator).ownBlockAt round =
      some (scheduledBlockRef validator round)
  /-- A completed block-send record persists while the epoch is active. -/
  sentOwnBlockPersists : ∀ validator round earlier later,
    earlier ≤ later →
    (∀ time, earlier ≤ time → time ≤ later →
      (trace time).epochActive = true) →
    ((trace earlier).validatorState validator).sentOwnBlockAt round = true →
    ((trace later).validatorState validator).sentOwnBlockAt round = true
  /-- Accepted representatives persist while the epoch is active. -/
  acceptedRepresentativePersists : ∀ observer round author reference earlier later,
    earlier ≤ later →
    (∀ time, earlier ≤ time → time ≤ later →
      (trace time).epochActive = true) →
    ((trace earlier).validatorState observer).acceptedRepresentative round author =
      some reference →
    ((trace later).validatorState observer).acceptedRepresentative round author =
      some reference
  /-- Correct validators use the canonical accepted representative. -/
  acceptedRepresentativeIsCanonical : ∀ time observer author round reference,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((trace time).validatorState observer).acceptedRepresentative round author =
      some reference →
    reference = scheduledBlockRef author round
  /-- An accepted representative is accepted in the local block store. -/
  representativeIsAccepted : ∀ time observer author round,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((trace time).validatorState observer).acceptedRepresentative round author =
      some (scheduledBlockRef author round) →
    ((trace time).validatorState observer).accepted
      (scheduledBlockRef author round) = true
  /-- Local block acceptance installs the canonical author representative. -/
  acceptedInstallsRepresentative : ∀ time observer author round,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((trace time).validatorState observer).accepted
      (scheduledBlockRef author round) = true →
    ((trace time).validatorState observer).acceptedRepresentative round author =
      some (scheduledBlockRef author round)
  /-- The author retains and locally accepts each durable own block. -/
  durableOwnBlockIsServable : ∀ time author round,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((trace time).validatorState author).ownBlockAt round =
      some (scheduledBlockRef author round) →
    ((trace time).validatorState author).retained
        (scheduledBlockRef author round) = true ∧
      ((trace time).validatorState author).accepted
        (scheduledBlockRef author round) = true
  /-- After a validator produces in a round, each missing correct block in that
  round stays a local synchronization goal. This rule reads only requester
  state. -/
  missingBlockIsRequested : ∀ time requester author round,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    requester ≠ author →
    (((trace time).validatorState requester).ownBlockAt round).isSome = true →
    ((trace time).validatorState requester).accepted
      (scheduledBlockRef author round) = false →
    ((trace time).validatorState requester).requested
      (scheduledBlockRef author round) = true
  /-- Once the exact-next action is enabled, it stays enabled until it runs. -/
  proposalEnabledUntilRun : ∀ validator targetRound start later,
    start ≤ later →
    (∀ time, start ≤ time → time ≤ later →
      (trace time).epochActive = true) →
    ExactNextProposalEnabledAt config faults trace validator targetRound start →
    ((trace later).validatorState validator).sentOwnBlockAt targetRound = false →
    ExactNextProposalEnabledAt config faults trace validator targetRound later

/-- Optional canonical genesis data for the constructed from-genesis proof.
Capsule-based recovery does not use this input. -/
structure CanonicalGenesisRules
    {CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)) : Prop where
  produced : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).ownBlockAt 0 =
        some (scheduledBlockRef validator 0) ∧
      ((trace time).validatorState validator).sentOwnBlockAt 0 = true
  accepted : ∀ time observer author,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((trace time).validatorState observer).acceptedRepresentative 0 author =
      some (scheduledBlockRef author 0)

/-- The only proposal progress input. A concrete proposal action that stays
enabled at one correct validator eventually executes. -/
structure FairExactNextProposalActions
    {CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)) : Prop where
  continuouslyEnabledProposalExecutes : ∀ validator targetRound start,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (∀ later,
      start ≤ later →
      ((trace later).validatorState validator).sentOwnBlockAt targetRound = false →
      ExactNextProposalEnabledAt config faults trace validator targetRound later) →
    ∃ finish,
      start ≤ finish ∧
      ((trace finish).validatorState validator).ownBlockAt targetRound =
          some (scheduledBlockRef validator targetRound) ∧
        ((trace finish).validatorState validator).sentOwnBlockAt targetRound = true

/-- Pointwise acceptance plus the local representation rule identifies the
accepted correct-author block. -/
theorem accepted_representative_of_every_correct_acceptance
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    (rules : ExactNextBlockProductionRules config faults trace)
    {time observer author round : Nat}
    (accepted : EveryCorrectAvailableValidatorAccepted faults (trace time) round)
    (observerInRange : observer < config.authorityCount)
    (observerLive : faults.correctAvailable observer = true)
    (authorInRange : author < config.authorityCount)
    (authorLive : faults.correctAvailable author = true) :
    ((trace time).validatorState observer).acceptedRepresentative round author =
      some (scheduledBlockRef author round) := by
  have representativeSome := accepted observer author observerInRange observerLive
    authorInRange authorLive
  cases representativeValue :
      ((trace time).validatorState observer).acceptedRepresentative round author with
  | none => simp [representativeValue] at representativeSome
  | some reference =>
      have canonical := rules.acceptedRepresentativeIsCanonical time observer
        author round reference observerInRange observerLive authorInRange
        authorLive representativeValue
      simpa [canonical] using representativeValue

/-- Completed correct-validator production persists in an active epoch. -/
theorem every_correct_available_production_persists
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    (rules : ExactNextBlockProductionRules config faults trace)
    {round earlier later : Nat}
    (earlierBeforeLater : earlier ≤ later)
    (active : ∀ time, earlier ≤ time → time ≤ later →
      (trace time).epochActive = true)
    (produced : EveryCorrectAvailableValidatorProduced faults
      (trace earlier) round) :
    EveryCorrectAvailableValidatorProduced faults (trace later) round := by
  intro validator validatorInRange validatorLive
  rcases produced validator validatorInRange validatorLive with
    ⟨ownSome, sent⟩
  cases ownValue : ((trace earlier).validatorState validator).ownBlockAt round with
  | none => simp [ownValue] at ownSome
  | some reference =>
      have ownLater := rules.ownBlockPersists validator round reference earlier
        later earlierBeforeLater active ownValue
      have sentLater := rules.sentOwnBlockPersists validator round earlier later
        earlierBeforeLater active sent
      exact ⟨by simp [ownLater], sentLater⟩

/-- Correct-to-correct accepted representatives persist in an active epoch. -/
theorem every_correct_available_acceptance_persists
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    (rules : ExactNextBlockProductionRules config faults trace)
    {round earlier later : Nat}
    (earlierBeforeLater : earlier ≤ later)
    (active : ∀ time, earlier ≤ time → time ≤ later →
      (trace time).epochActive = true)
    (accepted : EveryCorrectAvailableValidatorAccepted faults
      (trace earlier) round) :
    EveryCorrectAvailableValidatorAccepted faults (trace later) round := by
  intro observer author observerInRange observerLive authorInRange authorLive
  have exactEarlier := accepted_representative_of_every_correct_acceptance rules
    accepted observerInRange observerLive authorInRange authorLive
  have exactLater := rules.acceptedRepresentativePersists observer round author
    (scheduledBlockRef author round) earlier later earlierBeforeLater active
    exactEarlier
  simp [exactLater]

/-- Pointwise persistence lets Lean combine finitely many local completion times.
This helper does not use stake or a quorum predicate. -/
theorem eventually_every_selected_validator
    {configCount : Nat}
    (selected : VoterSet)
    (predicate : Nat → Time → Prop)
    (start : Time)
    (persists : ∀ validator earlier later,
      earlier ≤ later → predicate validator earlier → predicate validator later)
    (eventually : ∀ validator,
      validator < configCount →
      selected validator = true →
      ∃ finish, start ≤ finish ∧ predicate validator finish) :
    ∃ finish,
      start ≤ finish ∧
      ∀ validator,
        validator < configCount →
        selected validator = true →
        predicate validator finish := by
  induction configCount with
  | zero =>
      exact ⟨start, Nat.le_refl _, by intro validator validatorInRange; omega⟩
  | succ count inductionHypothesis =>
      have earlierEventually : ∀ validator,
          validator < count →
          selected validator = true →
          ∃ finish, start ≤ finish ∧ predicate validator finish := by
        intro validator validatorInRange validatorSelected
        exact eventually validator (by omega) validatorSelected
      rcases inductionHypothesis earlierEventually with
        ⟨earlierFinish, startBeforeEarlier, earlierDone⟩
      by_cases lastSelected : selected count = true
      · rcases eventually count (by omega) lastSelected with
          ⟨lastFinish, startBeforeLast, lastDone⟩
        let finish := max earlierFinish lastFinish
        refine ⟨finish, ?_, ?_⟩
        · exact Nat.le_trans startBeforeEarlier (Nat.le_max_left _ _)
        · intro validator validatorInRange validatorSelected
          by_cases isLast : validator = count
          · subst validator
            exact persists count lastFinish finish (Nat.le_max_right _ _) lastDone
          · exact persists validator earlierFinish finish (Nat.le_max_left _ _)
              (earlierDone validator (by omega) validatorSelected)
      · exact ⟨earlierFinish, startBeforeEarlier, by
          intro validator validatorInRange validatorSelected
          have notLast : validator ≠ count := by
            intro isLast
            subst validator
            exact lastSelected validatorSelected
          exact earlierDone validator (by omega) validatorSelected⟩

/-- Under the exact-next local rules, one correct, available validator eventually
produces the successor of a complete local round. -/
theorem correct_validator_eventually_produces_successor
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    {start round validator : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (producedAtStart :
      EveryCorrectAvailableValidatorProduced faults (trace start) round)
    (acceptedAtStart :
      EveryCorrectAvailableValidatorAccepted faults (trace start) round)
    (validatorInRange : validator < config.authorityCount)
    (validatorLive : faults.correctAvailable validator = true) :
    ∃ finish,
      start ≤ finish ∧
      ((trace finish).validatorState validator).ownBlockAt (round + 1) =
          some (scheduledBlockRef validator (round + 1)) ∧
        ((trace finish).validatorState validator).sentOwnBlockAt (round + 1) = true := by
  have ownRoundSome :=
    (producedAtStart validator validatorInRange validatorLive).1
  have ownRoundCanonical :
      ((trace start).validatorState validator).ownBlockAt round =
        some (scheduledBlockRef validator round) := by
    cases ownRoundValue :
        ((trace start).validatorState validator).ownBlockAt round with
    | none => simp [ownRoundValue] at ownRoundSome
    | some reference =>
        have canonical := rules.ownBlockIsCanonical start validator round reference
          validatorInRange validatorLive ownRoundValue
        simpa [canonical] using ownRoundValue
  have targetNotSentGivesEnabled :
      ((trace start).validatorState validator).sentOwnBlockAt (round + 1) = false →
      ExactNextProposalEnabledAt config faults trace validator (round + 1)
        start := by
    intro targetNotSent
    refine ⟨active start (Nat.le_refl _), validatorInRange, validatorLive, ?_⟩
    cases targetValue :
        ((trace start).validatorState validator).ownBlockAt (round + 1) with
    | some reference =>
        have canonical := rules.ownBlockIsCanonical start validator (round + 1)
          reference validatorInRange validatorLive targetValue
        right
        exact ⟨by simpa [canonical] using targetValue, targetNotSent⟩
    | none =>
        have signerAtRound :
            ((trace start).validatorState validator).highestSignedRound = round :=
          rules.missingSuccessorKeepsSignerAtCurrent start validator round
            validatorInRange validatorLive ownRoundCanonical targetValue
        have localParents : LocalCorrectParentQuorum config faults
            (trace start) validator round := by
          have common :=
            every_correct_available_validator_accepted_gives_common_layer config
              faults (trace start) round acceptedAtStart
          exact common validator validatorInRange validatorLive
        left
        refine ⟨rfl, ?_, ?_⟩
        · simp [ValidatorWorldState.nextRecoveryRound, signerAtRound]
        · simpa using localParents
  cases sentValue :
      ((trace start).validatorState validator).sentOwnBlockAt (round + 1) with
  | true =>
      have targetBlock := rules.sentOwnBlockRequiresOwnBlock start validator
        (round + 1) validatorInRange validatorLive sentValue
      exact ⟨start, Nat.le_refl _, targetBlock, sentValue⟩
  | false =>
      have enabledAtStart := targetNotSentGivesEnabled sentValue
      have continuouslyEnabled : ∀ later,
          start ≤ later →
          ((trace later).validatorState validator).sentOwnBlockAt (round + 1) =
            false →
          ExactNextProposalEnabledAt config faults trace validator (round + 1)
            later := by
        intro later startBeforeLater stillNotSent
        exact rules.proposalEnabledUntilRun validator (round + 1) start later
          startBeforeLater
          (by
            intro time _ _
            exact active time (by omega))
          enabledAtStart stillNotSent
      exact proposalFairness.continuouslyEnabledProposalExecutes validator
        (round + 1) start validatorInRange validatorLive continuouslyEnabled

/-- Finite validator enumeration combines the local proposal completions into
one time when every correct, available validator produced the successor round. -/
theorem every_correct_validator_eventually_produces_successor
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    {start round : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (producedAtStart :
      EveryCorrectAvailableValidatorProduced faults (trace start) round)
    (acceptedAtStart :
      EveryCorrectAvailableValidatorAccepted faults (trace start) round) :
    ∃ finish,
      start ≤ finish ∧
      EveryCorrectAvailableValidatorProduced faults (trace finish) (round + 1) := by
  let completed := fun validator time =>
    start ≤ time ∧
      ((trace time).validatorState validator).ownBlockAt (round + 1) =
          some (scheduledBlockRef validator (round + 1)) ∧
        ((trace time).validatorState validator).sentOwnBlockAt (round + 1) = true
  have completionPersists : ∀ validator earlier later,
      earlier ≤ later → completed validator earlier →
        completed validator later := by
    intro validator earlier later earlierBeforeLater completedEarlier
    constructor
    · exact Nat.le_trans completedEarlier.1 earlierBeforeLater
    · constructor
      · exact rules.ownBlockPersists validator (round + 1)
          (scheduledBlockRef validator (round + 1)) earlier later
          earlierBeforeLater
          (by
            intro time earlierBeforeTime _
            exact active time
              (Nat.le_trans completedEarlier.1 earlierBeforeTime))
          completedEarlier.2.1
      · exact rules.sentOwnBlockPersists validator (round + 1) earlier later
          earlierBeforeLater
          (by
            intro time earlierBeforeTime _
            exact active time
              (Nat.le_trans completedEarlier.1 earlierBeforeTime))
          completedEarlier.2.2
  have eachCompletes : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ∃ finish, start ≤ finish ∧ completed validator finish := by
    intro validator validatorInRange validatorLive
    rcases correct_validator_eventually_produces_successor rules
        proposalFairness active producedAtStart acceptedAtStart validatorInRange
        validatorLive with ⟨finish, startBeforeFinish, own, sent⟩
    exact ⟨finish, startBeforeFinish, startBeforeFinish, own, sent⟩
  rcases eventually_every_selected_validator faults.correctAvailable completed
      start completionPersists eachCompletes with
    ⟨finish, startBeforeFinish, allCompleted⟩
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro validator validatorInRange validatorLive
  have completedAtFinish := allCompleted validator validatorInRange validatorLive
  exact ⟨by simp [completedAtFinish.2.1], completedAtFinish.2.2⟩

/-- One correct requester eventually accepts one correct author's successor
block. The source is the author's retained block. The accepted preceding round
provides the finite causal prefix. -/
theorem correct_requester_eventually_accepts_successor
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    {start round author requester : Nat}
    (afterGst : syncNetwork.gst ≤ start)
    (authorBlock :
      ((trace start).validatorState author).ownBlockAt (round + 1) =
        some (scheduledBlockRef author (round + 1)))
    (acceptedParents :
      EveryCorrectAvailableValidatorAccepted faults (trace start) round)
    (requesterProduced :
      (((trace start).validatorState requester).ownBlockAt (round + 1)).isSome =
        true)
    (authorInRange : author < config.authorityCount)
    (authorLive : faults.correctAvailable author = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterLive : faults.correctAvailable requester = true) :
    ∃ finish,
      start ≤ finish ∧
      ((trace finish).validatorState requester).acceptedRepresentative
          (round + 1) author =
        some (scheduledBlockRef author (round + 1)) := by
  let item := scheduledBlockRef author (round + 1)
  have ownerServable := rules.durableOwnBlockIsServable start author (round + 1)
    authorInRange authorLive authorBlock
  have source : RetainedCorrectOwnerItem config faults
      (blockSyncTraceOfWorld trace requestCursor) author item start := by
    refine ⟨authorInRange, authorLive, ?_, ?_⟩
    · simpa [RetainedAt, blockSyncTraceOfWorld, item] using ownerServable.1
    · simpa [AcceptedAt, blockSyncTraceOfWorld, item] using ownerServable.2
  by_cases requesterAlreadyAccepted :
      ((trace start).validatorState requester).accepted item = true
  · have representative := rules.acceptedInstallsRepresentative start requester
      author (round + 1) requesterInRange requesterLive authorInRange authorLive
      (by simpa [item] using requesterAlreadyAccepted)
    exact ⟨start, Nat.le_refl _, representative⟩
  · have requesterNotAccepted :
        ((trace start).validatorState requester).accepted item = false := by
      cases value : ((trace start).validatorState requester).accepted item
      · rfl
      · exact False.elim (requesterAlreadyAccepted value)
    have requesterNeeds : requester ≠ author →
        NeededAt (blockSyncTraceOfWorld trace requestCursor) requester item start := by
      intro requesterIsNotOwner
      have requested := rules.missingBlockIsRequested start requester author
        (round + 1) requesterInRange requesterLive authorInRange authorLive
        requesterIsNotOwner requesterProduced
        (by simpa [item] using requesterNotAccepted)
      simpa [NeededAt, blockSyncTraceOfWorld] using requested
    have earlierAccepted : EarlierHistoryAccepted
        (blockSyncTraceOfWorld trace requestCursor) requester
        (scheduledBlock config faults author (round + 1)).parents start := by
      intro parent parentInHistory
      have positiveRound : 0 < round + 1 := by omega
      simp only [scheduledBlock, positiveRound, if_pos, List.mem_map]
        at parentInHistory
      rcases parentInHistory with
        ⟨parentAuthor, parentAuthorInList, parentIsCanonical⟩
      subst parent
      have parentFacts : parentAuthor < config.authorityCount ∧
          faults.correctAvailable parentAuthor = true := by
        simpa [scheduledCorrectValidators] using parentAuthorInList
      have representative :=
        accepted_representative_of_every_correct_acceptance rules
          acceptedParents requesterInRange requesterLive parentFacts.1
          parentFacts.2
      have accepted := rules.representativeIsAccepted start requester parentAuthor
        round requesterInRange requesterLive parentFacts.1 parentFacts.2
        representative
      simpa [AcceptedAt, blockSyncTraceOfWorld, item] using accepted
    have historyOrder :
        (scheduledBlock config faults author (round + 1)).parents ++ [item] =
          (scheduledBlock config faults author (round + 1)).parents ++ item :: [] :=
      rfl
    rcases retained_owner_history_item_eventually_accepted storage requests
        serving accepting source requesterInRange requesterLive afterGst
        requesterNeeds historyOrder earlierAccepted with
      ⟨finish, startBeforeFinish, acceptedAtFinish⟩
    have representative := rules.acceptedInstallsRepresentative finish requester
      author (round + 1) requesterInRange requesterLive authorInRange authorLive
      (by simpa [AcceptedAt, blockSyncTraceOfWorld, item] using acceptedAtFinish)
    exact ⟨finish, startBeforeFinish, representative⟩

/-- Finite requester enumeration gives one time when all correct, available
validators accepted one correct author's successor block. -/
theorem correct_author_successor_eventually_accepted_by_all
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    {start round author : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (afterGst : syncNetwork.gst ≤ start)
    (authorBlock :
      ((trace start).validatorState author).ownBlockAt (round + 1) =
        some (scheduledBlockRef author (round + 1)))
    (acceptedParents :
      EveryCorrectAvailableValidatorAccepted faults (trace start) round)
    (producedSuccessor :
      EveryCorrectAvailableValidatorProduced faults (trace start) (round + 1))
    (authorInRange : author < config.authorityCount)
    (authorLive : faults.correctAvailable author = true) :
    ∃ finish,
      start ≤ finish ∧
      ∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ((trace finish).validatorState requester).acceptedRepresentative
            (round + 1) author =
          some (scheduledBlockRef author (round + 1)) := by
  let acceptedBy := fun requester time =>
    start ≤ time ∧
      ((trace time).validatorState requester).acceptedRepresentative
          (round + 1) author =
        some (scheduledBlockRef author (round + 1))
  have acceptancePersists : ∀ requester earlier later,
      earlier ≤ later → acceptedBy requester earlier →
        acceptedBy requester later := by
    intro requester earlier later earlierBeforeLater acceptedEarlier
    constructor
    · exact Nat.le_trans acceptedEarlier.1 earlierBeforeLater
    · exact rules.acceptedRepresentativePersists requester (round + 1) author
        (scheduledBlockRef author (round + 1)) earlier later
        earlierBeforeLater
        (by
          intro time earlierBeforeTime _
          exact active time (Nat.le_trans acceptedEarlier.1 earlierBeforeTime))
        acceptedEarlier.2
  have eachRequesterAccepts : ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      ∃ finish, start ≤ finish ∧ acceptedBy requester finish := by
    intro requester requesterInRange requesterLive
    have requesterProduced :=
      (producedSuccessor requester requesterInRange requesterLive).1
    rcases correct_requester_eventually_accepts_successor requestCursor storage
        requests serving accepting rules afterGst authorBlock acceptedParents
        requesterProduced authorInRange authorLive requesterInRange requesterLive with
      ⟨finish, startBeforeFinish, acceptedAtFinish⟩
    exact ⟨finish, startBeforeFinish, startBeforeFinish, acceptedAtFinish⟩
  rcases eventually_every_selected_validator faults.correctAvailable acceptedBy
      start acceptancePersists eachRequesterAccepts with
    ⟨finish, startBeforeFinish, allAccepted⟩
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro requester requesterInRange requesterLive
  exact (allAccepted requester requesterInRange requesterLive).2

/-- Finite author enumeration gives one time when every correct, available
validator accepted every correct, available successor block. -/
theorem every_successor_block_eventually_accepted
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    {start round : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (afterGst : syncNetwork.gst ≤ start)
    (producedSuccessor :
      EveryCorrectAvailableValidatorProduced faults (trace start) (round + 1))
    (acceptedParents :
      EveryCorrectAvailableValidatorAccepted faults (trace start) round) :
    ∃ finish,
      start ≤ finish ∧
      EveryCorrectAvailableValidatorAccepted faults (trace finish) (round + 1) := by
  let authorAccepted := fun author time =>
    start ≤ time ∧
      ∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ((trace time).validatorState requester).acceptedRepresentative
            (round + 1) author =
          some (scheduledBlockRef author (round + 1))
  have authorAcceptancePersists : ∀ author earlier later,
      earlier ≤ later → authorAccepted author earlier →
        authorAccepted author later := by
    intro author earlier later earlierBeforeLater acceptedEarlier
    constructor
    · exact Nat.le_trans acceptedEarlier.1 earlierBeforeLater
    · intro requester requesterInRange requesterLive
      exact rules.acceptedRepresentativePersists requester (round + 1) author
        (scheduledBlockRef author (round + 1)) earlier later
        earlierBeforeLater
        (by
          intro time earlierBeforeTime _
          exact active time (Nat.le_trans acceptedEarlier.1 earlierBeforeTime))
        (acceptedEarlier.2 requester requesterInRange requesterLive)
  have eachAuthorAccepted : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ finish, start ≤ finish ∧ authorAccepted author finish := by
    intro author authorInRange authorLive
    have ownSome :=
      (producedSuccessor author authorInRange authorLive).1
    have authorBlock :
        ((trace start).validatorState author).ownBlockAt (round + 1) =
          some (scheduledBlockRef author (round + 1)) := by
      cases ownValue :
          ((trace start).validatorState author).ownBlockAt (round + 1) with
      | none => simp [ownValue] at ownSome
      | some reference =>
          have canonical := rules.ownBlockIsCanonical start author (round + 1)
            reference authorInRange authorLive ownValue
          simpa [canonical] using ownValue
    rcases correct_author_successor_eventually_accepted_by_all requestCursor
        storage requests serving accepting rules active afterGst authorBlock
        acceptedParents producedSuccessor authorInRange authorLive with
      ⟨finish, startBeforeFinish, acceptedAtFinish⟩
    exact ⟨finish, startBeforeFinish, startBeforeFinish, acceptedAtFinish⟩
  rcases eventually_every_selected_validator faults.correctAvailable authorAccepted
      start authorAcceptancePersists eachAuthorAccepted with
    ⟨finish, startBeforeFinish, allAuthorsAccepted⟩
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro observer author observerInRange observerLive authorInRange authorLive
  have exactRepresentative :=
    (allAuthorsAccepted author authorInRange authorLive).2 observer
      observerInRange observerLive
  simp [exactRepresentative]

/-- One complete correct round advances to its complete successor. Production is
from local exact-next proposal fairness. Acceptance is from retained-owner block
synchronization and the finite causal prefix. -/
theorem complete_correct_round_eventually_advances
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    {start round : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (afterGst : syncNetwork.gst ≤ start)
    (producedAtStart :
      EveryCorrectAvailableValidatorProduced faults (trace start) round)
    (acceptedAtStart :
      EveryCorrectAvailableValidatorAccepted faults (trace start) round) :
    ∃ finish,
      start ≤ finish ∧
      EveryCorrectAvailableValidatorProduced faults (trace finish) (round + 1) ∧
      EveryCorrectAvailableValidatorAccepted faults (trace finish) (round + 1) := by
  rcases every_correct_validator_eventually_produces_successor rules
      proposalFairness active producedAtStart acceptedAtStart with
    ⟨producedFinish, startBeforeProduced, producedSuccessor⟩
  have acceptedParentsAtProduced :=
    every_correct_available_acceptance_persists rules startBeforeProduced
      (by
        intro time _ _
        exact active time (by omega))
      acceptedAtStart
  have activeAfterProduced : ∀ time, producedFinish ≤ time →
      (trace time).epochActive = true := by
    intro time producedBeforeTime
    exact active time (Nat.le_trans startBeforeProduced producedBeforeTime)
  have syncAfterGst : syncNetwork.gst ≤ producedFinish :=
    Nat.le_trans afterGst startBeforeProduced
  rcases every_successor_block_eventually_accepted requestCursor storage requests
      serving accepting rules activeAfterProduced syncAfterGst producedSuccessor
      acceptedParentsAtProduced with
    ⟨finish, producedBeforeFinish, acceptedSuccessor⟩
  have producedAtFinish := every_correct_available_production_persists rules
    producedBeforeFinish
    (by
      intro time producedBeforeTime _
      exact activeAfterProduced time producedBeforeTime)
    producedSuccessor
  exact ⟨finish, Nat.le_trans startBeforeProduced producedBeforeFinish,
    producedAtFinish, acceptedSuccessor⟩

/-- A complete recovery base advances by each finite exact-next offset. The base
can come from a durable capsule. It does not have to be genesis. -/
theorem complete_base_round_eventually_reaches_offset
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    {start baseRound : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (afterGst : syncNetwork.gst ≤ start)
    (baseProduced :
      EveryCorrectAvailableValidatorProduced faults (trace start) baseRound)
    (baseAccepted :
      EveryCorrectAvailableValidatorAccepted faults (trace start) baseRound) :
    ∀ offset,
      ∃ finish,
        start ≤ finish ∧
        EveryCorrectAvailableValidatorProduced faults (trace finish)
          (baseRound + offset) ∧
        EveryCorrectAvailableValidatorAccepted faults (trace finish)
          (baseRound + offset) := by
  intro offset
  induction offset with
  | zero =>
      simpa using ⟨start, Nat.le_refl start, baseProduced, baseAccepted⟩
  | succ previous inductionHypothesis =>
      rcases inductionHypothesis with
        ⟨previousFinish, startBeforePrevious, previousProduced,
          previousAccepted⟩
      have activeAfterPrevious : ∀ time, previousFinish ≤ time →
          (trace time).epochActive = true := by
        intro time previousBeforeTime
        exact active time (Nat.le_trans startBeforePrevious previousBeforeTime)
      have previousAfterGst : syncNetwork.gst ≤ previousFinish :=
        Nat.le_trans afterGst startBeforePrevious
      rcases complete_correct_round_eventually_advances requestCursor storage
          requests serving accepting rules proposalFairness activeAfterPrevious
          previousAfterGst previousProduced previousAccepted with
        ⟨finish, previousBeforeFinish, produced, accepted⟩
      refine ⟨finish, Nat.le_trans startBeforePrevious previousBeforeFinish,
        ?_, ?_⟩ <;>
        simpa [Nat.add_assoc] using ‹_›

/-- Persistence combines all requested exact-next offsets at one finish time. -/
theorem complete_base_round_eventually_produces_window
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    {start baseRound : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (afterGst : syncNetwork.gst ≤ start)
    (baseProduced :
      EveryCorrectAvailableValidatorProduced faults (trace start) baseRound)
    (baseAccepted :
      EveryCorrectAvailableValidatorAccepted faults (trace start) baseRound)
    (count : Nat) :
    ∃ finish,
      start ≤ finish ∧
      ∀ offset,
        offset < count →
        EveryCorrectAvailableValidatorProduced faults (trace finish)
            (baseRound + offset) ∧
          EveryCorrectAvailableValidatorAccepted faults (trace finish)
            (baseRound + offset) := by
  let complete := fun offset time =>
    start ≤ time ∧
      EveryCorrectAvailableValidatorProduced faults (trace time)
          (baseRound + offset) ∧
        EveryCorrectAvailableValidatorAccepted faults (trace time)
          (baseRound + offset)
  have completionPersists : ∀ offset earlier later,
      earlier ≤ later → complete offset earlier → complete offset later := by
    intro offset earlier later earlierBeforeLater completedEarlier
    have activeBetween : ∀ time, earlier ≤ time → time ≤ later →
        (trace time).epochActive = true := by
      intro time earlierBeforeTime _
      exact active time (Nat.le_trans completedEarlier.1 earlierBeforeTime)
    exact ⟨Nat.le_trans completedEarlier.1 earlierBeforeLater,
      every_correct_available_production_persists rules earlierBeforeLater
        activeBetween completedEarlier.2.1,
      every_correct_available_acceptance_persists rules earlierBeforeLater
        activeBetween completedEarlier.2.2⟩
  have eachOffset : ∀ offset,
      offset < count →
      (fun candidate => decide (candidate < count)) offset = true →
      ∃ finish, start ≤ finish ∧ complete offset finish := by
    intro offset offsetInRange _selected
    rcases complete_base_round_eventually_reaches_offset requestCursor storage
        requests serving accepting rules proposalFairness active afterGst
        baseProduced baseAccepted offset with
      ⟨finish, startBeforeFinish, produced, accepted⟩
    exact ⟨finish, startBeforeFinish, startBeforeFinish, produced, accepted⟩
  rcases eventually_every_selected_validator (configCount := count)
      (fun offset => decide (offset < count)) complete start completionPersists
      eachOffset with ⟨finish, startBeforeFinish, allComplete⟩
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro offset offsetInRange
  have completed := allComplete offset offsetInRange (by simp [offsetInRange])
  exact ⟨completed.2.1, completed.2.2⟩

/-- Static genesis data gives the first complete correct round at every time. -/
theorem genesis_correct_round_is_complete
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    (rules : ExactNextBlockProductionRules config faults trace)
    (genesis : CanonicalGenesisRules config faults trace)
    (time : Nat) :
    EveryCorrectAvailableValidatorProduced faults (trace time) 0 ∧
      EveryCorrectAvailableValidatorAccepted faults (trace time) 0 := by
  constructor
  · intro validator validatorInRange validatorLive
    have genesisBlock := genesis.produced time validator validatorInRange
      validatorLive
    exact ⟨by simp [genesisBlock.1], genesisBlock.2⟩
  · intro observer author observerInRange observerLive authorInRange authorLive
    have genesisBlock := genesis.accepted time observer author observerInRange
      observerLive authorInRange authorLive
    simp [genesisBlock]

/-- Repeated local proposal and block-sync transitions reach every finite round.
No quorum layer or parent-availability result is an input. -/
theorem every_finite_correct_round_is_eventually_complete
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    (genesis : CanonicalGenesisRules config faults trace)
    {start : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (afterGst : syncNetwork.gst ≤ start) :
    ∀ round,
      ∃ finish,
        start ≤ finish ∧
        EveryCorrectAvailableValidatorProduced faults (trace finish) round ∧
        EveryCorrectAvailableValidatorAccepted faults (trace finish) round := by
  intro round
  induction round with
  | zero =>
      have genesisRound := genesis_correct_round_is_complete rules genesis start
      exact ⟨start, Nat.le_refl _, genesisRound⟩
  | succ parentRound inductionHypothesis =>
      rcases inductionHypothesis with
        ⟨parentFinish, startBeforeParent, producedParent, acceptedParent⟩
      have activeAfterParent : ∀ time, parentFinish ≤ time →
          (trace time).epochActive = true := by
        intro time parentBeforeTime
        exact active time (Nat.le_trans startBeforeParent parentBeforeTime)
      have parentAfterGst : syncNetwork.gst ≤ parentFinish :=
        Nat.le_trans afterGst startBeforeParent
      rcases complete_correct_round_eventually_advances requestCursor storage
          requests serving accepting rules proposalFairness activeAfterParent
          parentAfterGst producedParent acceptedParent with
        ⟨finish, parentBeforeFinish, producedSuccessor, acceptedSuccessor⟩
      exact ⟨finish, Nat.le_trans startBeforeParent parentBeforeFinish,
        producedSuccessor, acceptedSuccessor⟩

/-- Persistence combines the finite round results at one time. This gives a
complete prefix without using a common layer as an input. -/
theorem every_finite_correct_prefix_is_eventually_complete
    {CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId)}
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    (genesis : CanonicalGenesisRules config faults trace)
    {start : Nat}
    (active : ∀ time, start ≤ time → (trace time).epochActive = true)
    (afterGst : syncNetwork.gst ≤ start)
    (limit : Nat) :
    ∃ finish,
      start ≤ finish ∧
      ∀ round,
        round ≤ limit →
        EveryCorrectAvailableValidatorProduced faults (trace finish) round ∧
        EveryCorrectAvailableValidatorAccepted faults (trace finish) round := by
  let allRounds : VoterSet := fun _ => true
  let complete := fun round time =>
    start ≤ time ∧
      EveryCorrectAvailableValidatorProduced faults (trace time) round ∧
      EveryCorrectAvailableValidatorAccepted faults (trace time) round
  have completionPersists : ∀ round earlier later,
      earlier ≤ later → complete round earlier → complete round later := by
    intro round earlier later earlierBeforeLater completedEarlier
    have activeBetween : ∀ time, earlier ≤ time → time ≤ later →
        (trace time).epochActive = true := by
      intro time earlierBeforeTime _
      exact active time (Nat.le_trans completedEarlier.1 earlierBeforeTime)
    exact ⟨Nat.le_trans completedEarlier.1 earlierBeforeLater,
      every_correct_available_production_persists rules earlierBeforeLater
        activeBetween completedEarlier.2.1,
      every_correct_available_acceptance_persists rules earlierBeforeLater
        activeBetween completedEarlier.2.2⟩
  have eachRoundCompletes : ∀ round,
      round < limit + 1 →
      allRounds round = true →
      ∃ finish, start ≤ finish ∧ complete round finish := by
    intro round _roundInRange _selected
    rcases every_finite_correct_round_is_eventually_complete requestCursor storage
        requests serving accepting rules proposalFairness genesis active afterGst
        round with
      ⟨finish, startBeforeFinish, produced, accepted⟩
    exact ⟨finish, startBeforeFinish, startBeforeFinish, produced, accepted⟩
  rcases eventually_every_selected_validator (configCount := limit + 1)
      allRounds complete start completionPersists eachRoundCompletes with
    ⟨finish, startBeforeFinish, allComplete⟩
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro round roundBeforeLimit
  have completed := allComplete round (by omega) (by rfl)
  exact ⟨completed.2.1, completed.2.2⟩

/-- Every fair trace that follows the local exact-next and block-sync contracts
has per-validator unbounded block production. -/
theorem every_fair_exact_next_trace_has_block_production_liveness
    {CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage ScheduledBlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId))
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    (syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket)
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    (genesis : CanonicalGenesisRules config faults trace) :
    BlockProductionLiveness config faults network trace := by
  intro validator start minimumRound validatorInRange validatorLive _afterGst active
  let proofStart := max start syncNetwork.gst
  have startBeforeProof : start ≤ proofStart := Nat.le_max_left _ _
  have proofAfterSyncGst : syncNetwork.gst ≤ proofStart := Nat.le_max_right _ _
  have activeAfterProof : ∀ time, proofStart ≤ time →
      (trace time).epochActive = true := by
    intro time proofBeforeTime
    exact active time (Nat.le_trans startBeforeProof proofBeforeTime)
  let targetRound := max minimumRound
    (((trace start).validatorState validator).highestSignedRound + 1)
  rcases every_finite_correct_round_is_eventually_complete requestCursor storage
      requests serving accepting rules proposalFairness genesis activeAfterProof
      proofAfterSyncGst targetRound with
    ⟨finish, proofBeforeFinish, produced, _accepted⟩
  have validatorProduced := produced validator validatorInRange validatorLive
  refine ⟨finish, targetRound,
    Nat.le_trans startBeforeProof proofBeforeFinish, ?_, ?_, validatorProduced⟩
  · exact Nat.le_max_left _ _
  · dsimp [targetRound]
    have successorAtMost :
        ((trace start).validatorState validator).highestSignedRound + 1 ≤
          max minimumRound
            (((trace start).validatorState validator).highestSignedRound + 1) :=
      Nat.le_max_right _ _
    omega

/-- The same fair local execution gives every finite consecutive common window
used by the stalled-commit proof. The stalled-head input is not needed for block
production. -/
theorem every_fair_exact_next_trace_has_recovery_layer_window_liveness
    {CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage ScheduledBlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace
      (ValidatorWorldState ScheduledBlockId CommitId PacketId))
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlockRef ScheduledBlockId)) →
        Prop}
    (syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket)
    (requestCursor : Time → Nat → Nat)
    (storage : BlockSyncStorageRules
      (blockSyncTraceOfWorld trace requestCursor))
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      (blockSyncTraceOfWorld trace requestCursor))
    (rules : ExactNextBlockProductionRules config faults trace)
    (proposalFairness : FairExactNextProposalActions config faults trace)
    (genesis : CanonicalGenesisRules config faults trace) :
    RecoveryLayerWindowLiveness config faults network trace := by
  intro start minimumRound count baseline _afterGst active _stalled
  let proofStart := max start syncNetwork.gst
  have startBeforeProof : start ≤ proofStart := Nat.le_max_left _ _
  have proofAfterSyncGst : syncNetwork.gst ≤ proofStart := Nat.le_max_right _ _
  have activeAfterProof : ∀ time, proofStart ≤ time →
      (trace time).epochActive = true := by
    intro time proofBeforeTime
    exact active time (Nat.le_trans startBeforeProof proofBeforeTime)
  let limit := minimumRound + count
  rcases every_finite_correct_prefix_is_eventually_complete requestCursor storage
      requests serving accepting rules proposalFairness genesis activeAfterProof
      proofAfterSyncGst limit with
    ⟨finish, proofBeforeFinish, completePrefix⟩
  refine ⟨finish, minimumRound,
    Nat.le_trans startBeforeProof proofBeforeFinish, Nat.le_refl _, ?_⟩
  intro offset offsetInWindow
  have roundBeforeLimit : minimumRound + offset ≤ limit := by
    dsimp [limit]
    exact Nat.add_le_add_left (Nat.le_of_lt offsetInWindow) minimumRound
  have completeRound := completePrefix (minimumRound + offset) roundBeforeLimit
  have produced := completeRound.1
  have accepted := completeRound.2
  exact ⟨produced, accepted,
    every_correct_available_validator_produced_gives_quorum_layer config faults
      (trace finish) (minimumRound + offset) produced,
    every_correct_available_validator_accepted_gives_common_layer config faults
      (trace finish) (minimumRound + offset) accepted⟩

end Mysticeti
